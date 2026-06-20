<?php

function foo(string $s): void {
    // Under the `@` error-suppression operator, Psalm widens destructured
    // targets that aren't guaranteed present with null: the first element of a
    // non-empty-list is still string, but every later element becomes nullable.
    @[$a, $b] = explode(":", $s);
    /** @psalm-check-type-exact $a = string */;
    /** @psalm-check-type-exact $b = string|null */;
}
