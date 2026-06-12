<?php
class Foo
{
    /**
     * @psalm-suppress InvalidConstantAssignmentValue
     *
     * @var int
     */
    public const BAR = "bar";
}
