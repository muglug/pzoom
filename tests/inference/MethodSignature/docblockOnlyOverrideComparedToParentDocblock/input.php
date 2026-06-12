<?php

class BaseHandler {
    /**
     * @param non-empty-list<string> $command
     */
    protected function restart(array $command): void
    {
        echo count($command);
    }
}

class ChildRestarter extends BaseHandler {
    /**
     * No type hint to allow handler v1 and v2 usage
     *
     * @param non-empty-list<string> $command
     */
    protected function restart($command): void
    {
        echo count($command);
    }
}
