<?php
interface S {
    public function __toString(): string;
}
/** @return array<array-key, string> */
function f(S $s): array {
    return [$s];
}
                
