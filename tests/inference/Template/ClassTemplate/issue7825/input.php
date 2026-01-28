<?php
interface Stub {}
interface ProxyQueryInterface {}
class MockObject {}
/** @phpstan-template T of ProxyQueryInterface */
interface PagerInterface {}
/** @phpstan-template T of ProxyQueryInterface */
class Datagrid
{
    /** @var T */
    private $query;

    /** @var PagerInterface<T> */
    private $pager;

    /**
     * @phpstan-param T                 $query
     * @phpstan-param PagerInterface<T> $pager
     */
    public function __construct(
        ProxyQueryInterface $query,
        PagerInterface $pager
    ) {
        $this->pager = $pager;
        $this->query = $query;
    }
}
interface FormBuilderInterface {}
/** @template T of FieldDescriptionInterface */
class FieldDescriptionCollection {}
interface FieldDescriptionInterface {}
abstract class Test
{
    /** @var Datagrid<ProxyQueryInterface&Stub> */
    private Datagrid $datagrid;

    /** @var PagerInterface<ProxyQueryInterface&Stub>&MockObject */
    private $pager;

    /** @var ProxyQueryInterface&Stub */
    private $query;

    /** @var FieldDescriptionCollection<FieldDescriptionInterface> */
    private FieldDescriptionCollection $columns;

    private FormBuilderInterface $formBuilder;

    /**
     * @psalm-template RealInstanceType of object
     * @psalm-param class-string<RealInstanceType> $originalClassName
     * @psalm-return MockObject&RealInstanceType
     */
    abstract protected function createMock(string $originalClassName): MockObject;

    /**
     * @psalm-template RealInstanceType of object
     * @psalm-param    class-string<RealInstanceType> $originalClassName
     * @psalm-return   Stub&RealInstanceType
     */
    abstract protected function createStub(string $originalClassName): Stub;

    protected function setUp(): void
    {
        $this->query = $this->createStub(ProxyQueryInterface::class);
        $this->columns = new FieldDescriptionCollection();

        /** @var PagerInterface<ProxyQueryInterface&Stub>&MockObject $pager */
        $pager = $this->createMock(PagerInterface::class);
        $this->pager = $pager;
        $this->datagrid = new Datagrid($this->query, $pager);
    }
}
