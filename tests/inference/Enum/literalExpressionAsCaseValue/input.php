<?php
enum Mask: int {
    case One = 1 << 0;
    case Two = 1 << 1;
}
$z = Mask::Two->value;
