<?php
interface I {
    public function test1(bool $idOnly): array;
}

class Test implements I
{
    /**
     * @template T1 as bool
     * @param T1 $idOnly
     * @psalm-return (T1 is true ? array : array)
     */
    public function test1(bool $idOnly): array {
        return [];
    }
}