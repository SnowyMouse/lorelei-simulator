# Lorelei Simulator

Simulate thousands of Pokémon Gen 1-2 AI move decisions.

## GUI version

This isn't available yet. See here: https://github.com/SnowyMouse/lorelei-simulator/issues/2

## Command-line tool

What you need:
* a ROM (English Pokémon Red, Blue, Yellow, Gold, Silver, and Crystal are all 
  supported)
* a save state (supports SameBoy as well as any BESS-compatible save states)

Note: This save state must be made just before the AI begins making a decision. 

* In Gen 1, the AI decides the moment you select a move, thus you can make your
  save state in the FIGHT dialogue with your cursor highlighting a move.
* In Gen 2, the AI decides just before you have the option to select a move. As
  such, you need to make your save state either when a Pokémon is doing its cry
  (in the case a Pokémon is being switched), or at the very end of a turn.

Note that Gen 2 is considerably slower than Gen 1: https://github.com/SnowyMouse/lorelei-simulator/issues/1

Then, open a command-line and run the following command:
```shell
lorelei_simulator_cli path/to/rom path/to/savestate
```

You can add additional parameters:
* `-j <JOBS>` to specify thread count (by default it will use however many
  logical processors your CPU has)
* `-t <TRIALS>` to limit how many trials to calculate (by default, it will keep
  going until you press CTRL-C)
* `-q` to not print anything until finished (by default, you will see a live
  update)

Provided you give a correct ROM and save state, you will see the output in a
table.
