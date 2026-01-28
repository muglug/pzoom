<?php
class Subject{}

/**
 * @template U of Subject
 * @template-extends EventAbstract<U>
 */
class EventInstance extends EventAbstract
{
    /**
     * @template S of Subject
     * @param S $subject
     * @return self<S>
     */
    public static function createInstance(Subject $subject) : self
    {
        return new self($subject);
    }
}

/**
 * @template T of Subject
 */
abstract class EventAbstract
{
    /** @var T */
    protected $subject;

    /**
     * @param T $subject
     */
    public function __construct(Subject $subject)
    {
        $this->subject = $subject;
    }
}