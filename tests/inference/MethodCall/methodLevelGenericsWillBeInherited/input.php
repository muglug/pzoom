<?php
interface I
{
    /**
     * @template TResult
     * @param TResult $value
     * @return TResult
     */
    public function method(mixed $value): mixed;
}
final class A implements I
{
    public function method(mixed $value): mixed
    {
        return $value;
    }
}
$_v = (new A)->method("a");
/** @psalm-check-type-exact $_v = "a" */
