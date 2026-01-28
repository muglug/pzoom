<?php
function getIntOrNull(): ?int{return null;}
$a = getIntOrNull();

if ($a < 0) {
    echo $a + 3;
}

if ($a <= 0) {
    /** @psalm-suppress PossiblyNullOperand */
    echo $a + 3;
}

if ($a > 0) {
    echo $a + 3;
}

if ($a >= 0) {
    /** @psalm-suppress PossiblyNullOperand */
    echo $a + 3;
}

if (0 < $a) {
    echo $a + 3;
}

if (0 <= $a) {
    /** @psalm-suppress PossiblyNullOperand */
    echo $a + 3;
}

if (0 > $a) {
    echo $a + 3;
}

if (0 >= $a) {
    /** @psalm-suppress PossiblyNullOperand */
    echo $a + 3;
}
