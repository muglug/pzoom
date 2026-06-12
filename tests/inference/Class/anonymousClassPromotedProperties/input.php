<?php
interface Factory { public function make(): string; }
final class Hub { public function id(): string { return 'h'; } }

function f(): Factory {
    return new class() implements Factory {
        public function __construct(
            private readonly Hub $hub = new Hub(),
        ) {
        }
        public function make(): string {
            return $this->hub->id();
        }
    };
}
