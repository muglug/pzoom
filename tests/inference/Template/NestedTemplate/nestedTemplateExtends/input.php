<?php
namespace Foo;

interface IBaseViewData {}

/**
 * @template TViewData
 */
class BaseModel {}

/**
 * @template TViewData as IBaseViewData
 * @template TModel as BaseModel<TViewData>
 */
abstract class BaseRepository {}

class StudentViewData implements IBaseViewData {}
class TeacherViewData implements IBaseViewData {}

/** @extends BaseModel<StudentViewData> */
class StudentModel extends BaseModel {}

/** @extends BaseModel<TeacherViewData> */
class TeacherModel extends BaseModel {}

/** @extends BaseRepository<StudentViewData, StudentModel> */
class StudentRepository extends BaseRepository {}

/** @extends BaseRepository<TeacherViewData, TeacherModel> */
class TeacherRepository extends BaseRepository {}