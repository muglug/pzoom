<?php
/** @param ArrayAccess<int, string> $a */
function f(ArrayAccess $a): void {
    echo $a->offsetGet(offset: 0);
}
