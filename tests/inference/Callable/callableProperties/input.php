<?php
class C {
    /** @psalm-var callable():bool */
    private $callable;

    /**
     * @psalm-param callable():bool $callable
     */
    public function __construct(callable $callable) {
        $this->callable = $callable;
    }

    public function callTheCallableDirectly(): bool {
        return ($this->callable)();
    }

    public function callTheCallableIndirectly(): bool {
        $r = $this->callable;
        return $r();
    }
}
