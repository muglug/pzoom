<?php
interface S {
    public function __toString(): string;
}
/** @return array{string} */
function f(S $s): array {
    return [$s];
}
                
