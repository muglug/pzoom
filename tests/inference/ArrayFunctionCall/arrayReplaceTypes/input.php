<?php
/**
 * @psalm-type A=array{name: string}
 * @psalm-type B=array{age: int}
 */
class Demo
{
    /**
     * @param A $a
     * @param B $b
     * @return A&B
     */
    public function replace($a, $b): array
    {
        return array_replace($a, $b);
    }
}
