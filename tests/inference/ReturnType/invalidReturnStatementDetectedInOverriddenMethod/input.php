<?php
/** @template T */
interface I
{
    /** @return T */
    public function process(): mixed;
}
/** @implements I<int> */
final class B implements I
{
    public function process(): mixed
    {
        return '';
    }
}
