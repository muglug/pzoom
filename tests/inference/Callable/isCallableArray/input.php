<?php
class A
{
    public function callMeMaybe(string $method): void
    {
        $handleMethod = [$this, $method];

        if (is_callable($handleMethod)) {
            $handleMethod();
        }
    }

    public function foo(): void {}
}
$a = new A();
$a->callMeMaybe("foo");
