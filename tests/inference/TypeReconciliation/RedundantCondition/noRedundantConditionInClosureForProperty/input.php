<?php
class Queue {
    private bool $closed = false;

    public function enqueue(string $value): Closure {
        if ($this->closed) {
            return function() : void {
                if ($this->closed) {}
            };
        }

        return function() : void {};
    }
}