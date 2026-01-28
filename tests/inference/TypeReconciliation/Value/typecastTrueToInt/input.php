<?php
/**
* @param 0|1 $int
*/
function foo(int $int) : void {
    echo (string) $int;
}

foo((int) true);
