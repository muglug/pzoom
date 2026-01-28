<?php
/** @param mixed $a */
function a($a): void{
    if (is_numeric($a)) {
        assert(!is_float($a));
    }
}
/** @param mixed $a */
function b($a): void{
    if (is_numeric($a)) {
        assert(!is_int($a));
    }
}