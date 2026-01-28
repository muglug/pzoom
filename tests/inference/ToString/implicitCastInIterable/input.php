<?php
interface S {
    public function __toString(): string;
}
/** @return iterable<int, string> */
function f(S $s) {
    return [$s];
}
                
