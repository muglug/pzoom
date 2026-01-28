<?php
/**
 * @template TResult as string|list<string>
 * @param TResult $result
 * @return (TResult is array ? list<string> : string)
 */
function recursion($result) {
    if (\is_array($result)) {
        return $result;
    }

    return strtoupper($result);
}
