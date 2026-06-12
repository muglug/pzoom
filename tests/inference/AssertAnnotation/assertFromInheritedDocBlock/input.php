<?php
    namespace Namespace1 {

    /** @template InstanceType */
    interface PluginManagerInterface
    {
        /** @psalm-assert InstanceType $value */
        public function validate(mixed $value): void;
    }

    /**
     * @template InstanceType
     * @template-implements PluginManagerInterface<InstanceType>
     */
    abstract class AbstractPluginManager implements PluginManagerInterface
    {
    }

    /**
     * @template InstanceType of object
     * @template-extends AbstractPluginManager<InstanceType>
     */
    abstract class AbstractSingleInstancePluginManager extends AbstractPluginManager
    {
        public function validate(mixed $value): void
        {
        }
    }
}

namespace Namespace2 {
    use InvalidArgumentException;use Namespace1\AbstractSingleInstancePluginManager;
    use Namespace1\AbstractPluginManager;
    use stdClass;

    /** @template-extends AbstractSingleInstancePluginManager<stdClass> */
    final class Qoo extends AbstractSingleInstancePluginManager
    {
    }

    /** @template-extends AbstractPluginManager<callable> */
    final class Ooq extends AbstractPluginManager
    {
        public function validate(mixed $value): void
        {
        }
    }
}

namespace {
    $baz = new \Namespace2\Qoo();

    /** @var mixed $object */
    $object = null;
    $baz->validate($object);

    $ooq = new \Namespace2\Ooq();
    /** @var mixed $callable */
    $callable = null;
    $ooq->validate($callable);
}
