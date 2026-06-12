<?php
/**
 * @psalm-suppress RawObjectIteration
 * @psalm-suppress MixedAssignment
 */
function foo(array $a) : void {
    if (isset($a["a"]["b"])) {
        foreach ($a["a"] as $c) {}
    }
}
