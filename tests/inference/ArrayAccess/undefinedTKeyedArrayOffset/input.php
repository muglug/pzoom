<?php
class Example {
    /**
     * @param array{a: string, b: int} $context
     */
    function foo(array $context): void {
        $context["c"];
    }
}
