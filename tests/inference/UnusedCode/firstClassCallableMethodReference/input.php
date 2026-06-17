<?php

final class Handler
{
    public function instanceHandler(): void {}

    public static function staticHandler(): void {}

    private function privateHandler(): void {}

    public function run(): void
    {
        $callbacks = [
            $this->instanceHandler(...),
            self::staticHandler(...),
            $this->privateHandler(...),
        ];

        foreach ($callbacks as $cb) {
            $cb();
        }
    }
}

(new Handler())->run();
