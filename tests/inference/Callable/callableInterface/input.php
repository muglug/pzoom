<?php
interface CallableInterface{
    public function __invoke(): bool;
}

function takesInvokableInterface(CallableInterface $c): void{
    takesCallable($c);
}

function takesCallable(callable $c): void {
    $c();
}
