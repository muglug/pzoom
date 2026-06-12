<?php
enum Mask: int {
    case One = 1 << 0;
    case Two = 1 << 1;
}
/** @return Mask */
function a() {
    return Mask::One;
}

$z = a()->value;
