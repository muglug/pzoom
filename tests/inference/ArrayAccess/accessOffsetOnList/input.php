<?php
/** @param list<int> $arr */
function foo(array $arr) : void {
    echo $arr[3] ?? null;
}
