<?php

function onKernelController(callable $controller): void
{
    if (\is_array($controller)) {
        $controller = $controller[0];
    } else {
        try {
            $reflection = new \ReflectionFunction($controller(...));
            $controller = $reflection->getClosureThis();
        } catch (\ReflectionException) {
            return;
        }
    }
}
