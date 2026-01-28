<?php
/**
 * @template T of Enum
 */
class EnumSet
{
    /**
     * @var class-string<T>
     */
    private $type;

    /**
     * @param class-string<T> $type
     */
    public function __construct(string $type)
    {
        $this->type = $type;
    }
}

abstract class Enum {
    /**
     * @return EnumSet<static>
     */
    public static function all()
    {
        return new EnumSet(static::class);
    }
}

/**
 * @extends EnumSet<CustomEnum>
 */
final class CustomEnumSet extends EnumSet {
    public function __construct()
    {
        parent::__construct(CustomEnum::class);
    }
}

class CustomEnum extends Enum
{
    public static function all()
    {
        return new EnumSet(static::class);
    }
}