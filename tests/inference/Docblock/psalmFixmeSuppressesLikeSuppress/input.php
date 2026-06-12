<?php
/**
 * @psalm-fixme MixedArgument
 */
function f(array $a): void {
    echo strtolower($a[0]);
}

function g(array $a): void {
    /** @psalm-fixme MixedArgument */
    echo strtolower($a[0]);
}

function h(array $a): void {
    echo strtolower($a[0]);
}
