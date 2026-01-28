<?php
/**
 * @return Iterator<string>
 */
function buildIterator(int $size): Iterator {

    $values = [];
    for ($i = 0;  $i < $size; $i++) {
       $values[] = "Item $i\n";
    }

    return new ArrayIterator($values);
}

$it = buildIterator(2);

if ($it->current() === null) {}
