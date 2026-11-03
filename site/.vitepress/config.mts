import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'ru-RU',
  base: process.env.SITE_BASE || '/',
  title: 'OmniView',
  description: 'Экраны, сообщения и задания в компьютерном классе',
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    logo: '/brand-mark.svg',
    siteTitle: 'OmniView',
    nav: [
      { text: 'Возможности', link: '/features' },
      { text: 'Пример урока', link: '/demo' },
      { text: 'Быстрый старт', link: '/quick-start' },
      { text: 'Legacy', link: '/legacy' },
      { text: 'Скачать', link: '/download' }
    ],
    sidebar: [
      { text: 'Обзор', items: [{ text: 'Главная', link: '/' }, { text: 'Пример урока', link: '/demo' }] },
      { text: 'Работа', items: [{ text: 'Возможности', link: '/features' }, { text: 'Быстрый старт', link: '/quick-start' }] },
      { text: 'Для разработчика', items: [{ text: 'Архитектура', link: '/architecture' }, { text: 'Сборка и тесты', link: '/development' }] },
      { text: 'Релизы', items: [{ text: 'Скачать', link: '/download' }, { text: 'Обновления', link: '/updates' }, { text: 'Legacy: другие системы', link: '/legacy' }] }
    ],
    footer: { message: 'OmniView · локальный компьютерный класс', copyright: 'OmniView — подразделение OmniCONTROL' },
    search: { provider: 'local' }
  },
  vite: { resolve: { alias: { '@site': '/site' } } }
})
