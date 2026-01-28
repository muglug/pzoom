<?php
interface I {
    /**
     * @param bool $b
     */
    public function foo($b): bool;
}

class T implements I
{
    /**
     * @template TBool as bool
     * @param TBool $b
     *
     * @psalm-return TBool
     */
    public function foo($b): bool {
        return $b;
    }
}