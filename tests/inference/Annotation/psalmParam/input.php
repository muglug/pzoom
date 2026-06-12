<?php
function takesInt(int $_a): void {}

/**
 * @psalm-param  array<int, string> $a
 * @param string[] $a
 */
function foo(array $a): void {
    foreach ($a as $key => $_value) {
        takesInt($key);
    }
}
