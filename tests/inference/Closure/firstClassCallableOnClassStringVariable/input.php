<?php

class Ev {}

interface ProviderIface {
    public static function check(Ev $event): ?bool;
}

class Registry {
    /** @param Closure(Ev): ?bool $c */
    public function registerClosure(Closure $c): void {}

    /** @param class-string $class */
    public function registerClass(string $class): void
    {
        if (is_subclass_of($class, ProviderIface::class, true)) {
            $callable = $class::check(...);
            $this->registerClosure($callable);
        }
    }
}
