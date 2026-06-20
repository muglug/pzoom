<?php

function foo(string $s): void {
    // explode(...) with the default limit returns non-empty-list<string>, so
    // both positional targets are a guaranteed string (Psalm: $a, $b = string).
    [$a, $b] = explode(":", $s);
    /** @psalm-check-type-exact $a = string */;
    /** @psalm-check-type-exact $b = string */;
}
