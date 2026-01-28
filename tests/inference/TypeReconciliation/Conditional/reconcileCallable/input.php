<?php
function reflectCallable(callable $callable): ReflectionFunctionAbstract {
    if (\is_array($callable)) {
        return new \ReflectionMethod($callable[0], $callable[1]);
    } elseif ($callable instanceof \Closure || \is_string($callable)) {
        return new \ReflectionFunction($callable);
    } else {
        return new \ReflectionMethod($callable, "__invoke");
    }
}