<?php
function foo(array $array): void {}

/** @var list<array> $arrays */
$arrays = [];
foreach (array_column($arrays, null, "name") as $array) {
    foo($array);
}
            
