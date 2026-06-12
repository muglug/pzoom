<?php
class A
{
    public function foo(): void {}

    public function getMeAgain(): void {
        /** @var self */
        $me = $this;
        $me->foo();
    }
}
