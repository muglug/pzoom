<?php
enum Mask: int {
    case One = 1 << 0;
    case Two = 1 << 1;
    case Four = 1 << 2;
}
/** @return Mask::One|Mask::Two */
function a() {
    return Mask::One;
}

$z = a()->value;
