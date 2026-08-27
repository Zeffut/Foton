#!/bin/bash
# Runs every in-world test once, in sequence, and reports the tally.
#
# They are run one at a time on purpose: they each start a server on their own
# port but several share a run directory naming pattern, and two at once tread
# on each other.
cd "$(dirname "$0")/.." || exit 1
export PATH="$HOME/.cargo/bin:$PATH"

pass=0
fail=0
failed=""
for test in dev/join-test.sh dev/selector-test.sh dev/rcon-test.sh \
            dev/summon-test.sh \
            dev/sapling-test.sh dev/container-test.sh \
            dev/chest-loot-test.sh \
            dev/block-components-test.sh \
            dev/flowerpot-test.sh dev/enderchest-test.sh dev/spawnegg-test.sh \
            dev/interact-test.sh \
            dev/boat-test.sh dev/openers-test.sh dev/ride-test.sh \
            dev/mount-test.sh \
            dev/minecart-test.sh dev/jukebox-test.sh dev/frame-test.sh \
            dev/throw-test.sh dev/melee-test.sh dev/fall-test.sh \
            dev/workstation-test.sh dev/beacon-test.sh \
            dev/beehive-test.sh dev/tnt-minecart-test.sh \
            dev/furnace-minecart-test.sh dev/hopper-minecart-test.sh \
            dev/decoration-test.sh \
            dev/moss-test.sh dev/structure-block-test.sh \
            dev/fire-test.sh dev/grass-test.sh dev/death-test.sh \
            dev/map-test.sh \
            dev/lightning-test.sh \
            dev/dragon-test.sh \
            dev/leash-test.sh dev/happy-ghast-test.sh \
            dev/shelf-test.sh dev/campfire-test.sh dev/conduit-test.sh \
            dev/dripstone-test.sh dev/tnt-test.sh \
            dev/spawner-test.sh dev/catalyst-test.sh \
            dev/sculk-vibration-test.sh dev/warden-test.sh \
            dev/fishing-test.sh \
            dev/nautilus-test.sh \
            dev/villager-day-test.sh \
            dev/raid-test.sh \
            dev/nether-test.sh dev/reload-test.sh; do
  name=$(basename "$test")
  if bash "$test" > "/tmp/$name.out" 2>&1; then
    printf '%-24s PASS\n' "$name"
    pass=$((pass + 1))
  else
    printf '%-24s FAIL\n' "$name"
    fail=$((fail + 1))
    failed="$failed $name"
  fi
done

echo "=============================="
echo "passed $pass, failed $fail"
if [ -n "$failed" ]; then
  echo "failed:$failed"
  exit 1
fi
