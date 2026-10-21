"""Phase 7: exports each trained class's models (python/models/<class>/) to the game's
ONNX models in assets/ai/:
- <class>-recruit.onnx: its first duel snapshot, lightly trained, for recruits;
- <class>-veteran.onnx: its melee model, the most advanced, for veterans.
A model takes one observation [1, observation_size] and gives one logit per action
[1, action_count]; the likeliest is played. `--version` goes to the ONNX model_version,
and the class, level and training steps to its metadata.

From the repo root inside python/.venv: python python/export.py --version 1
"""

import argparse
from pathlib import Path

import numpy as np
import onnx
import torch
from onnx.reference import ReferenceEvaluator
from stable_baselines3 import PPO

from lanista_env import FightEnv
from train import MODELS

ASSETS = Path(__file__).parent.parent / "assets" / "ai"


class Logits(torch.nn.Module):
    """The policy's action logits for an observation."""

    def __init__(self, policy):
        super().__init__()
        self.policy = policy

    def forward(self, observation):
        policy = self.policy
        features = policy.extract_features(observation, policy.pi_features_extractor)
        return policy.action_net(policy.mlp_extractor.forward_actor(features))


def export(model_path, path, version, metadata):
    model = PPO.load(model_path, device="cpu")
    size = model.observation_space.shape[0]
    torch.onnx.export(
        Logits(model.policy).eval(),
        torch.zeros(1, size),
        path,
        input_names=["observation"],
        output_names=["logits"],
        opset_version=17,
        dynamo=False,
    )
    exported = onnx.load(path)
    exported.model_version = version
    metadata["steps"] = model.num_timesteps
    for key, value in metadata.items():
        exported.metadata_props.add(key=key, value=str(value))
    onnx.save(exported, path)
    # Check: the exported model picks the policy's likeliest action.
    observations = np.random.default_rng(0).uniform(-1, 1, (20, size)).astype(np.float32)
    evaluator = ReferenceEvaluator(exported)
    for observation in observations:
        (logits,) = evaluator.run(None, {"observation": observation[None]})
        assert logits.argmax() == model.predict(observation, deterministic=True)[0], path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", type=int, required=True)
    args = parser.parse_args()
    ASSETS.mkdir(parents=True, exist_ok=True)
    for fighter_class in FightEnv().env.class_keys:
        models = MODELS / fighter_class
        snapshots = sorted(
            (models / "duel").glob("rl_model_*_steps.zip"),
            key=lambda p: int(p.stem.split("_")[2]),
        )
        if not snapshots or not (models / "melee.zip").exists():
            print(f"{fighter_class}: not trained, skipped")
            continue
        for level, model_path in [("recruit", snapshots[0]), ("veteran", models / "melee.zip")]:
            path = ASSETS / f"{fighter_class}-{level}.onnx"
            export(model_path, path, args.version, {"class": fighter_class, "level": level})
            print(f"{path.name}: version {args.version}, from {model_path.name}")


if __name__ == "__main__":
    main()
