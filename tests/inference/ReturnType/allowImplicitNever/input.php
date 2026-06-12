<?php
class TestCase
{
    /** @psalm-return never-return */
    public function markAsSkipped(): void
    {
        throw new \Exception();
    }
}
class A extends TestCase
{
    /**
     * @return string[]
     */
    public function foo(): array
    {
        $this->markAsSkipped();
    }
}

class B extends A
{
    public function foo(): array
    {
        return ["foo"];
    }
}
