<?php
interface S {
    public function __toString(): string;
}
/** @return array{0:string} */
function f(S $s): array {
    return [$s];
}
                
