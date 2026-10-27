"""Phase 7: Gymnasium environments around lanista_py's Environment.

The agent plays fighter 0 of a duel or of a melee of 3 to 20 generated
fighters with `melee=True`; `opponent` (observations -> actions) plays all the others,
a random action each by default; `utility_ai` leaves them to the utility AI. With
`fighter_class` (a class key), the agent is always of that class. The episode ends for the agent once it is out of the fight. `vector_env(n)` runs n fights
in parallel processes. `review` plays one fight and gives its log to read.

Self-check, from the repo root inside python/.venv: python python/lanista_env.py
"""

import gymnasium as gym
import numpy as np

import lanista_py

# ponytail: the melee's largest size, as `melee.max_fighters` in data/combat.ron.
MAX_MELEE = 20


def utility_ai(observations):
    """Leaves every opponent to the utility AI."""
    return [None] * len(observations)


class FightEnv(gym.Env):
    def __init__(self, opponent=None, melee=False, fighter_class=None):
        self.env = lanista_py.Environment()
        self.melee = melee
        assert fighter_class in [None, *self.env.class_keys], fighter_class
        self.fighter_class = fighter_class
        self.observation_space = gym.spaces.Box(-1.0, 1.0, (self.env.observation_size,), np.float32)
        self.action_space = gym.spaces.Discrete(self.env.action_count)
        self.opponent = opponent or (
            lambda observations: [self.action_space.sample() for _ in observations]
        )

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        if seed is not None:
            self.action_space.seed(seed)
        # Fights are drawn until the agent is of its class.
        while True:
            fight = int(self.np_random.integers(2**63))
            if self.melee:
                count = int(self.np_random.integers(3, MAX_MELEE + 1))
                observations = self.env.reset_melee(fight, count)
            else:
                observations = self.env.reset(fight)
            if self.fighter_class in (None, self.env.classes[0]):
                break
        if hasattr(self.opponent, "new_fight"):
            self.opponent.new_fight(self.np_random)
        self.others = observations[1:]
        return np.array(observations[0], np.float32), {}

    def step(self, action):
        actions = [int(action)] + list(self.opponent(self.others))
        observations, rewards, done = self.env.step(actions)
        self.others = observations[1:]
        terminated = self.env.over or not self.env.fighting[0]
        truncated = done and not terminated
        return np.array(observations[0], np.float32), rewards[0], terminated, truncated, {}


def vector_env(n, opponent=None, melee=False):
    """n fights in parallel processes; `opponent` must pickle."""
    return gym.vector.AsyncVectorEnv([lambda: FightEnv(opponent, melee)] * n)


def review(policy, seed, opponent=None, melee=False):
    """Plays the fight of `seed` with `policy` (observation -> action) as fighter 0;
    gives its log, report and the agent's total reward, to read for absurd behaviour."""
    env = FightEnv(opponent, melee)
    observation, _ = env.reset(seed=seed)
    total, done = 0.0, False
    while not done:
        observation, reward, terminated, truncated, _ = env.step(policy(observation))
        total += reward
        done = terminated or truncated
    return f"{env.env.log()}\nagent's total reward: {total:.3f}"


if __name__ == "__main__":
    from gymnasium.utils.env_checker import check_env

    check_env(FightEnv(), skip_render_check=True)
    check_env(FightEnv(utility_ai, melee=True), skip_render_check=True)
    augure = FightEnv(fighter_class="augure")
    for seed in range(5):
        augure.reset(seed=seed)
        assert augure.env.classes[0] == "augure"
    envs = vector_env(4)
    envs.reset(seed=7)
    episodes = 0
    while episodes < 8:
        _, _, terminated, truncated, _ = envs.step(envs.action_space.sample())
        episodes += int(np.sum(terminated | truncated))
    envs.close()
    random_agent = FightEnv().action_space.sample
    assert "agent's total reward" in review(lambda _: random_agent(), 7)
    assert "agent's total reward" in review(lambda _: random_agent(), 7, utility_ai, True)
    print(
        "ok: check_env passed for a duel and a melee, the agent's class kept, 8 duels played on 4 processes, two reviewed"
    )
