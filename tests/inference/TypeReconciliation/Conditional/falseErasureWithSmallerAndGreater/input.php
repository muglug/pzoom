<?php
/** @return int|false */
function getIntOrFalse() {return false;}
$a = getIntOrFalse();

if ($a < 0) {
    echo $a + 3;
}

if ($a <= 0) {
    /** @psalm-suppress PossiblyFalseOperand */
    echo $a + 3;
}

if ($a > 0) {
    echo $a + 3;
}

if ($a >= 0) {
    /** @psalm-suppress PossiblyFalseOperand */
    echo $a + 3;
}

if (0 < $a) {
    echo $a + 3;
}

if (0 <= $a) {
    /** @psalm-suppress PossiblyFalseOperand */
    echo $a + 3;
}

if (0 > $a) {
    echo $a + 3;
}

if (0 >= $a) {
    /** @psalm-suppress PossiblyFalseOperand */
    echo $a + 3;
}
