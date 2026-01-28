<?php
if (class_exists(A::class)) {
    if (method_exists(A::class, "method")) {
        /** @psalm-suppress MixedArgument */
        echo A::method();
    }

    echo A::class;
    /** @psalm-suppress MixedArgument */
    echo A::SOME_CONST;
}
