<?php
/**
 * @template TCallback as Closure():string
 */
class A {
    /** @var TCallback */
    private $callback;

    /** @param TCallback $callback */
    public function __construct(Closure $callback) {
        $this->callback = $callback;
    }

    /** @param TCallback $callback */
    public function setCallback(Closure $callback): void {
        $this->callback = $callback;
    }
}
$a = new A(function() { return "a";});
$a->setCallback(function() { return "b";});
