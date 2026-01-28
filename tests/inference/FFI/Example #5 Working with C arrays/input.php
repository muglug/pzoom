<?php
// create C data structure
$a = FFI::new("long[1024]");
// work with it like with a regular PHP array
$size = count($a);
for ($i = 0; $i < $size; $i++) {
    $a[$i] = $i;
}
$sum = 0;
/** @psalm-suppress MixedAssignment */
foreach ($a as $n) {
    /** @psalm-suppress MixedOperand */
    $sum += $n;
}
                
