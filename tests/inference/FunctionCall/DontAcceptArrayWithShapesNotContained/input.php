<?php

/** @param array{bar: 0|positive-int} $foo */
function takesArrayShapeWithZeroOrPositiveInt(array $foo): void
{
}

/** @var int $mayBeInt */
$mayBeInt = -1;

takesArrayShapeWithZeroOrPositiveInt(["bar" => $mayBeInt]);
                
