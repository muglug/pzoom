<?php
function foo(callable $c): void {}

/** @psalm-suppress MissingParamType */
function bar($a, $b) : void {
    foo([$a, $b]);
}
