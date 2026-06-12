<?php
/**
 * @psalm-type _Type A::TYPE_*
 */
class A{
    const TYPE_A = 1;
    const TYPE_B = 2;

    public function f(): void {
        /** @psalm-var _Type $var*/
        static $var;
    }
}
