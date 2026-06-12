<?php
/** @param list<callable> $listeners
 *  @param list<mixed> $arguments */
function f(array $listeners, array $arguments): void
{
    foreach ($listeners as $listener) {
        /** @psalm-suppress MixedAssignment */
        $result = call_user_func_array($listener, $arguments);
        if ($result === false) {
            return;
        }
    }
}
