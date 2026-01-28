<?php
interface I {
    /**
     * @return int
     */
    public function check();
}

class C implements I
{
    /**
     * @psalm-suppress ImplementedReturnTypeMismatch
     */
    public function check(): bool
    {
        return false;
    }
}
