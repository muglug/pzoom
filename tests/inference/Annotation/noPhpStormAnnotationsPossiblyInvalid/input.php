<?php
/** @param ArrayIterator|string[] $i */
function takesArrayIteratorOfString($i): void {
    $s = $i->offsetGet("a");
}
