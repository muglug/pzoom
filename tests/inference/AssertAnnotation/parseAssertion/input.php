<?php
/**
 * @psalm-assert array<string, string[]> $data
 * @param mixed $data
 */
function isArrayOfStrings($data): void {}

function foo(array $arr) : void {
    isArrayOfStrings($arr);
    foreach ($arr as $a) {
        foreach ($a as $b) {
            echo $b;
        }
    }
}
