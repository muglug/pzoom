<?php
/**
 * @template T of DateTimeInterface
 * @param T $x
 * @return T
 */
function foo($x)
{
    return new \DateTime();
}
