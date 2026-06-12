<?php
/** @psalm-type _B=array{p1:string} */
function e(array $a): void
{
    /** @var _B $elt */
    foreach ($a as $elt) {
        echo $elt["p1"];
    }
}
