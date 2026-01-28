<?php
/**
 * @see https://github.com/vimeo/psalm/issues/8932
 *
 * @param array|null $value
 *
 * @return null
 */
function reverseTransform($value)
{
    if (null === $value) {
        return null;
    }

    if (!\is_array($value)) {
        throw new \Exception("array");
    }

    return null;
}
