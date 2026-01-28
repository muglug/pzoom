<?php
class S {
    const A = 0;
    const B = 1;
    const C = 2;
}

function foo(array $arr) : void {
    switch ($arr[S::A]) {
        case S::B:
        case S::C:
        break;
    }
}