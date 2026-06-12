<?php
class Foo {
    private string $status = "";

    public function assertValidStatusTransition(string $status): void
    {
        if (
            ("canceled" === $this->status && "complete" === $status)
            || ("canceled" === $this->status && "pending" === $status)
            || ("complete" === $this->status && "canceled" === $status)
            || ("complete" === $this->status && "pending" === $status)
        ) {
            throw new \LogicException();
        }
    }
}
