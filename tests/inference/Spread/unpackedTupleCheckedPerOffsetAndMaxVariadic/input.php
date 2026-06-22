<?php

function twoTypes(int $id, string $name): void {}

// A tuple spread is checked element-by-element against the parameter at each
// position (Psalm's per-offset unpack handling): offset 0 ('x') fills the int
// parameter and is an InvalidScalarArgument; offset 1 fills the string and is
// fine. This is the function-call path (not just constructors).
function checksPerOffset(): void
{
    $pair = ['x', 'name'];
    twoTypes(...$pair);
}

// Spreading a list into max() unpacks into positional values, so the variadic
// max(mixed, mixed, ...) form applies — no false positive about max expecting a
// non-empty-array.
function maxSpread(): int
{
    return max(...[3, 1, 2]);
}
