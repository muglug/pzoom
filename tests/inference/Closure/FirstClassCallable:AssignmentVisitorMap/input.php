<?php
class Test {
    /** @var list<\Closure():void> */
    public array $handlers = [];

    public function register(): void {
        foreach ([1, 2, 3] as $index) {
            $this->push($this->handler(...));
        }
    }

    /**
     * @param Closure():void $closure
     * @return void
     */
    private function push(\Closure $closure): void {
        $this->handlers[] = $closure;
    }

    private function handler(): void {
    }
}

$test = new Test();
$test->register();
$handlers = $test->handlers;
