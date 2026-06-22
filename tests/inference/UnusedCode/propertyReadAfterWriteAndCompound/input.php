<?php

// A property read after being written in the same scope, and a property read
// via a compound assignment, are both uses: neither is reported unused.

final class Accumulator
{
    public array $items = [];
    private string $log = '';

    public function run(): int
    {
        $this->items += [1, 2];      // compound assignment reads $items
        $this->log = 'started';      // write...
        return strlen($this->log)    // ...then read (served from in-scope cache)
            + count($this->items);
    }
}

echo (new Accumulator())->run();
