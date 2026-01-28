<?php
/** @template-covariant T as object **/
interface Viewable
{
    /** @psalm-return T **/
    public function view(): object;
}

class CatView
{
    /**
      * @var string
      * @readonly
      */
    public $name;

    public function __construct(string $name) {
        $this->name = $name;
    }
}

/** @implements Viewable<CatView> */
class Cat implements Viewable
{
    public function view(): object {
        return new CatView("Kittie");
    }
}

/** @psalm-param Viewable<object> $viewable */
function getView(Viewable $viewable): object {
    return $viewable->view();
}

getView(new Cat());