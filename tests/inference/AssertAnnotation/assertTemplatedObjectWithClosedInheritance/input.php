<?php
/**
 * @template-covariant E
 * @template-covariant A
 * @psalm-inheritors Left<E> | Right<A>
 */
interface Either {
    /** @psalm-assert-if-true Left<E> $this */
    public function isLeft(): bool;

    /** @psalm-assert-if-true Right<A> $this */
    public function isRight(): bool;
}

/**
 * @template E
 * @implements Either<E, never>
 */
final class Left implements Either {
    public function isLeft(): bool
    {
        return true;
    }
    public function isRight(): bool
    {
        return false;
    }
}

/**
 * @template A
 * @implements Either<never, A>
 */
final class Right implements Either {
    public function isLeft(): bool
    {
        return false;
    }
    public function isRight(): bool
    {
        return true;
    }
}

/**
 * @template E
 * @template A
 * @param Either<E, A> $either
 * @psalm-assert-if-true Left<E> $either
 */
function isLeft(Either $either): bool
{
    return $either instanceof Left;
}

/**
 * @template E
 * @template A
 * @param Either<E, A> $either
 * @psalm-assert-if-true Right<A> $either
 */
function isRight(Either $either): bool
{
    return $either instanceof Right;
}

/**
 * @return Either<OutOfRangeException, int>
 */
function getEither(): Either
{
    throw new RuntimeException("???");
}

/**
 * @param Left<OutOfRangeException> $_left
 */
function testLeft(Left $_left): void {}

/**
 * @param Right<int> $_right
 */
function testRight(Right $_right): void {}

/** @param Either<OutOfRangeException, int> $either */
function isLeftFunctionIfElse(Either $either): void
{
    if (isLeft($either)) {
        /** @psalm-check-type-exact $either = Left<OutOfRangeException> */
        testLeft($either);
    } else {
        /** @psalm-check-type-exact $either = Right<int> */
        testRight($either);
    }
}

/** @param Either<OutOfRangeException, int> $either */
function isRightFunctionIfElse(Either $either): void
{
    if (isRight($either)) {
        testRight($either);
        /** @psalm-check-type-exact $either = Right<int> */
    } else {
        /** @psalm-check-type-exact $either = Left<OutOfRangeException> */
        testLeft($either);
    }
}

/** @param Either<OutOfRangeException, int> $either */
function testRightFunctionTernary(Either $either): void
{
    isRight($either) ? testRight($either) : testLeft($either);
}

/** @param Either<OutOfRangeException, int> $either */
function testLeftFunctionTernary(Either $either): void
{
    isLeft($either) ? testLeft($either) : testRight($either);
}

/** @param Either<OutOfRangeException, int> $either */
function isLeftMethodIfElse(Either $either): void
{
    if ($either->isLeft()) {
        /** @psalm-check-type-exact $either = Left<OutOfRangeException> */
        testLeft($either);
    } else {
        /** @psalm-check-type-exact $either = Right<int> */
        testRight($either);
    }
}

/** @param Either<OutOfRangeException, int> $either */
function isRightMethodIfElse(Either $either): void
{
    if ($either->isRight()) {
        testRight($either);
        /** @psalm-check-type-exact $either = Right<int> */
    } else {
        /** @psalm-check-type-exact $either = Left<OutOfRangeException> */
        testLeft($either);
    }
}

/** @param Either<OutOfRangeException, int> $either */
function testRightMethodTernary(Either $either): void
{
    $either->isRight() ? testRight($either) : testLeft($either);
}

/** @param Either<OutOfRangeException, int> $either */
function testLeftMethodTernary(Either $either): void
{
    $either->isLeft() ? testLeft($either) : testRight($either);
}
