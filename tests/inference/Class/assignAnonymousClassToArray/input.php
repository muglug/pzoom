<?php
/**
 * @param array<string, object> $array
 */
function foo(array $array, string $key) : void {
    foreach ($array as $i => $item) {
        $array[$key] = new class() {};

        if ($array[$i] === $array[$key]) {}
    }
}
