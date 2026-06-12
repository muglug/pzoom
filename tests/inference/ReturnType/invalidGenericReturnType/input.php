<?php
/** @return ArrayIterator<int, string> */
function foo(array $a) {
    $obj = new ArrayObject([1, 2, 3, 4]);
    return $obj->getIterator();
}
