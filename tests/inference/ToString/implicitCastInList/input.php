<?php
interface S {
    public function __toString(): string;
}
/** @return list<string> */
function f(S $s): array {
    return [$s];
}
                
