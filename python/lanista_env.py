"""Phase 7: Gymnasium environments around lanista_py's Environment.

The agent plays fighter 0 of a standard duel; `opponent` (observation -> action)
plays fighter 1, a random action by default. `vector_env(n)` runs n duels in
parallel processes.

Self-check, from the repo root inside python/.venv: python python/lanista_env.py
"""

import gymnasium as gym
import numpy as np

import lanista_py


class DuelEnv(gym.Env):
    def __init__(self, opponent=None):
        self.env = lanista_py.Environment()
        self.observation_space = gym.spaces.Box(
            -1.0, 1.0, (self.env.observation_size,), np.float32
        )
        self.action_space = gym.spaces.Discrete(self.env.action_count)
        self.opponent = opponent or (lambda _: self.action_space.sample())

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        if seed is not None:
            self.action_space.seed(seed)
        observations = self.env.reset(int(self.np_random.integers(2**63)))
        self.opponent_observation = observations[1]
        return np.array(observations[0], np.float32), {}

    def step(self, action):
        actions = [int(action), int(self.opponent(self.opponent_observation))]
        observations, rewards, done = self.env.step(actions)
        self.opponent_observation = observations[1]
        terminated = self.env.over
        truncated = done and not terminated
        return np.array(observations[0], np.float32), rewards[0], terminated, truncated, {}


def vector_env(n, opponent=None):
    """n duels in parallel processes; `opponent` must pickle."""
    return gym.vector.AsyncVectorEnv([lambda: DuelEnv(opponent)] * n)


if __name__ == "__main__":
    from gymnasium.utils.env_checker import check_env

    check_env(DuelEnv(), skip_render_check=True)
    envs = vector_env(4)
    envs.reset(seed=7)
    episodes = 0
    while episodes < 8:
        _, _, terminated, truncated, _ = envs.step(envs.action_space.sample())
        episodes += int(np.sum(terminated | truncated))
    envs.close()
    print("ok: check_env passed, 8 duels played on 4 processes")
