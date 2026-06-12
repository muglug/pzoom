<?php
class Test {
    const CONST1 = 1;

    public function test(): void
    {
        /** @var static::CONST1 */
        $a = static::CONST1;
    }
}
