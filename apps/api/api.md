# FloatCTF API 路由大纲

## 目录结构
```
/api
├── /admin
│   ├── /users
│   ├── /challenges
│   ├── /super_admin
│   ├── /instances
│   ├── /events
│   │   ├── /{event_id}/users
│   │   ├── /{event_id}/teams
│   │   ├── /{event_id}/challenges
│   │   ├── /{event_id}/announcements
│   │   └── /{event_id}/writeups
│   └── /system
└── /service
    ├── /users
    ├── /challenges
    ├── /instances
    ├── /solves
    ├── /events
    ├── /submit
    └── /super_admin
```

## 管理员API (/api/admin)

### 系统信息
- `GET /system/info` - 获取系统信息

### 用户管理
- `POST /users` - 创建用户
- `DELETE /users/{user_id}` - 删除用户
- `PUT /users/{user_id}` - 更新用户信息
- `PATCH /users/{user_id}` - 部分更新用户信息
- `GET /users` - 获取用户列表
- `GET /users/{user_id}` - 获取特定用户信息

### 挑战管理
- `POST /challenges/check` - 检查挑战配置
- `POST /challenges/web_import` - 从Web导入挑战
- `POST /challenges` - 创建挑战
- `DELETE /challenges/{challenge_id}` - 删除挑战
- `PUT /challenges/{challenge_id}` - 更新挑战
- `PATCH /challenges/{challenge_id}` - 部分更新挑战
- `GET /challenges` - 获取挑战列表
- `GET /challenges/{challenge_id}` - 获取特定挑战

### 超级管理员管理
- `POST /super_admin` - 创建超级管理员
- `DELETE /super_admin/{admin_id}` - 删除超级管理员
- `PUT /super_admin/{admin_id}` - 更新超级管理员
- `PATCH /super_admin/{admin_id}` - 部分更新超级管理员
- `GET /super_admin` - 获取超级管理员列表
- `GET /super_admin/{admin_id}` - 获取特定超级管理员

### 实例管理
- `GET /instances` - 获取实例列表
- `GET /instances/{instance_id}` - 获取特定实例

### 赛事管理
- `POST /events` - 创建赛事
- `DELETE /events/{event_id}` - 删除赛事
- `PUT /events/{event_id}` - 更新赛事
- `PATCH /events/{event_id}` - 部分更新赛事
- `GET /events` - 获取赛事列表
- `GET /events/{event_id}` - 获取特定赛事
- `GET /events/{event_id}/data` - 获取赛事数据

#### 赛事用户管理
- `POST /events/{event_id}/users` - 添加用户到赛事
- `DELETE /events/{event_id}/users/{user_id}` - 从赛事中移除用户
- `POST /events/{event_id}/users/{user_id}/banned` - 禁用赛事用户
- `DELETE /events/{event_id}/users/{user_id}/banned` - 解禁赛事用户
- `GET /events/{event_id}/users` - 获取赛事用户列表

#### 赛事团队管理
- `POST /events/{event_id}/teams` - 创建赛事团队
- `DELETE /events/{event_id}/teams/{team_id}` - 删除赛事团队
- `GET /events/{event_id}/teams` - 获取赛事团队列表
- `GET /events/{event_id}/teams/{team_id}/members` - 获取团队成员
- `POST /events/{event_id}/teams/{team_id}/members` - 添加用户到团队
- `DELETE /events/{event_id}/teams/{team_id}/members/{user_id}` - 从团队中移除用户

#### 赛事挑战管理
- `POST /events/{event_id}/challenges` - 添加挑战到赛事
- `DELETE /events/{event_id}/challenges/{challenge_id}` - 从赛事中移除挑战
- `GET /events/{event_id}/challenges` - 获取赛事挑战列表
- `POST /events/{event_id}/challenges/hidden` - 隐藏赛事挑战
- `DELETE /events/{event_id}/challenges/hidden` - 公开赛事挑战

#### 赛事公告管理
- `POST /events/{event_id}/announcements` - 添加赛事公告
- `PUT /events/{event_id}/announcements/{announcement_id}` - 更新赛事公告
- `DELETE /events/{event_id}/announcements/{announcement_id}` - 删除赛事公告
- `GET /events/{event_id}/announcements/{announcement_id}` - 获取特定赛事公告
- `GET /events/{event_id}/announcements` - 获取赛事公告列表

#### 赛事Writeup管理
- `GET /events/{event_id}/writeups` - 获取赛事所有Writeup

## 服务API (/api/service)

### 用户认证
- `POST /users/session` - 用户登录
- `POST /users` - 创建用户

### 超级管理员认证
- `POST /super_admin/admin/session` - 超级管理员登录

### 挑战访问
- `GET /challenges` - 获取挑战列表
- `GET /challenges/{challenge_id}` - 获取特定挑战
- `GET /challenges/{challenge_id}/instance` - 获取挑战实例

### 实例管理
- `GET /instances` - 获取实例列表
- `GET /instances/{instance_id}` - 获取特定实例
- `POST /instances/{instance_id}/launch` - 启动实例
- `DELETE /instances/{instance_id}/destroy` - 销毁实例

### 提交管理
- `POST /submit/flag` - 提交Flag
- `POST /submit/writeup` - 提交Writeup

### 解题记录
- `GET /solves` - 获取解题记录
- `GET /solves/top` - 获取前15名用户

### 赛事访问
- `GET /events` - 获取赛事列表
- `GET /events/{event_id}/challenges` - 获取赛事挑战
- `GET /events/{event_id}` - 获取特定赛事
- `GET /events/{event_id}/instances` - 获取赛事实例
- `GET /events/{event_id}/challenges/{challenge_id}/instance` - 获取赛事挑战实例
- `GET /events/{event_id}/scoreboard` - 获取赛事排行榜
- `GET /events/{event_id}/announcements` - 获取赛事公告
- `GET /events/{event_id}/trend` - 获取赛事趋势
- `POST /events/{event_id}/join` - 加入赛事
- `GET /events/{event_id}/submit/wp/status` - 获取提交Writeup状态
- `POST /events/{event_id}/leave` - 离开赛事