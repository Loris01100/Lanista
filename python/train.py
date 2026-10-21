"""Phase 7: trains the learned AI of one class with PPO (Stable-Baselines3), duel first,
then melee.

Each fight's opponents are drawn at its start (`Opponents`): the latest snapshot of the
model being trained (self-play), an older snapshot, or the utility AI, a third each,
the utility AI alone until the first snapshot. The melee stage starts from the duel
model, which joins its older versions. At the end, the model plays duels against the
utility AI and prints its win rate.

From the repo root inside python/.venv (pip install stable-baselines3), for each class:
    python python/train.py mirmillon duel --steps 2000000
    python python/train.py mirmillon melee --steps 2000000
Models and snapshots go to python/models/<class>/ (not committed); export.py turns them
into the game's ONNX models.
Self-check: python python/train.py augure duel --steps 4096 --envs 2 --check 20
"""

import argparse
import shutil
from pathlib import Path

import numpy as np
from stable_baselines3 import PPO
from stable_baselines3.common.callbacks import CheckpointCallback
from stable_baselines3.common.vec_env import SubprocVecEnv

from lanista_env import FightEnv, utility_ai

MODELS = Path(__file__).parent / "models"


class Opponents:
    """Plays every opponent of a fight with one policy drawn in `snapshots` at its start."""

    def __init__(self, snapshots):
        self.snapshots = snapshots
        self.loaded = {}
        self.policy = None

    def new_fight(self, rng):
        saved = sorted(self.snapshots.glob("*.zip"), key=lambda p: p.stat().st_mtime)
        draw = rng.random()
        if not saved or draw < 1 / 3:
            self.policy = None
            return
        path = saved[-1] if draw < 2 / 3 else saved[rng.integers(len(saved))]
        # ponytail: every snapshot drawn stays loaded; drop old ones if memory runs short.
        if path not in self.loaded:
            self.loaded[path] = PPO.load(path, device="cpu")
        self.policy = self.loaded[path]

    def __call__(self, observations):
        if self.policy is None or not observations:
            return utility_ai(observations)
        actions, _ = self.policy.predict(np.array(observations, np.float32))
        return [int(a) for a in actions]


def win_rate(model, fights, fighter_class):
    """Share of `fights` the model wins against the utility AI, playing its likeliest action."""
    env = FightEnv(utility_ai, fighter_class=fighter_class)
    wins = 0
    for seed in range(fights):
        observation, _ = env.reset(seed=seed)
        done = False
        while not done:
            action, _ = model.predict(observation, deterministic=True)
            observation, _, terminated, truncated, _ = env.step(action)
            done = terminated or truncated
        wins += int(env.env.over and env.env.fighting[0])
    return wins / fights


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("fighter_class", metavar="class", help="a class key of data/classes.ron")
    parser.add_argument("stage", choices=["duel", "melee"])
    parser.add_argument("--steps", type=int, default=2_000_000)
    parser.add_argument("--envs", type=int, default=8)
    parser.add_argument("--snapshot-every", type=int, default=100_000, help="steps")
    parser.add_argument("--check", type=int, default=1000, help="duels against the utility AI")
    args = parser.parse_args()

    melee = args.stage == "melee"
    models = MODELS / args.fighter_class
    snapshots = models / args.stage
    snapshots.mkdir(parents=True, exist_ok=True)
    # ponytail: the snapshots play every opponent, of any class, though trained for one.
    opponents = Opponents(snapshots)
    envs = SubprocVecEnv([lambda: FightEnv(opponents, melee, args.fighter_class)] * args.envs)
    envs.seed(0)
    if melee:
        shutil.copy(models / "duel.zip", snapshots / "duel.zip")
        model = PPO.load(models / "duel.zip", envs, device="cpu")
    else:
        model = PPO("MlpPolicy", envs, device="cpu", verbose=1)
    every = max(args.snapshot_every // args.envs, 1)
    model.learn(args.steps, CheckpointCallback(every, snapshots), reset_num_timesteps=not melee)
    model.save(models / args.stage)
    envs.close()
    rate = win_rate(model, args.check, args.fighter_class)
    print(f"{args.fighter_class} {args.stage}: {rate:.1%} of {args.check} duels won against the utility AI")


if __name__ == "__main__":
    main()
