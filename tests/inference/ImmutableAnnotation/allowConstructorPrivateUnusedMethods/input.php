<?php
/**
 * @psalm-immutable
 */
class PaymentShared
{
    /** @var int */
    private $commission;

    public function __construct()
    {
        $this->test();
        $this->commission = 1;
    }

    private function test(): void {}
}
