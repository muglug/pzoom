<?php
abstract class A {
    /** @psalm-param mixed $other */
    public function equals($other): void {
        if ($other instanceof $this) {
            /** @psalm-check-type $other = A&static */
            return;
        }
    }
}
