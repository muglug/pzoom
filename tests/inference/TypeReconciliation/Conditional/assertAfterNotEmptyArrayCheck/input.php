<?php
function foo(array $c): void {
    if (!empty($c["d"])) {}

    foreach (["a", "b", "c"] as $k) {
        /** @psalm-suppress MixedAssignment */
        foreach ($c[$k] as $d) {}
    }
}