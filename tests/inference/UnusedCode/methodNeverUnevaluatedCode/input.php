<?php
final class A{
    /** @return never */
    function neverReturns() {
        die();
    }

    function f(): void {
        $this->neverReturns();
        echo "hello";
    }
}

