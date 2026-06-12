<?php

class Event {}

interface AfterIface {
    public static function go(Event $event): void;
}

class D {
    /** @var list<class-string<AfterIface>> */
    private array $checks = [];

    public function dispatch(Event $event): void
    {
        foreach ($this->checks as $handler) {
            $handler::go($event);
        }
    }
}
